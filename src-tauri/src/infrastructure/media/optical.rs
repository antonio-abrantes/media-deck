//! Windows IMAPI 2 adapter. COM objects never leave the calling thread and no
//! command shell is involved.

use crate::domain::errors::DomainError;
use crate::domain::optical::{
    OpticalCancel, OpticalPhase, OpticalPort, OpticalProgress, OpticalProgressSink, OpticalRecorder,
};
use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use windows::core::{implement, Interface, Ref, BSTR};
use windows::Win32::Foundation::{E_NOTIMPL, VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::Storage::Imapi::{
    DDiscFormat2DataEvents, DDiscFormat2DataEvents_Impl, FsiFileSystemISO9660, FsiFileSystemJoliet,
    FsiFileSystems, IDiscFormat2Data, IDiscFormat2DataEventArgs, IDiscFormat2Erase, IDiscMaster2,
    IDiscRecorder2, IFileSystemImage, MsftDiscFormat2Data, MsftDiscFormat2Erase, MsftDiscMaster2,
    MsftDiscRecorder2, MsftFileSystemImage, IMAPI_FORMAT2_DATA_MEDIA_STATE_BLANK,
    IMAPI_FORMAT2_DATA_MEDIA_STATE_WRITE_PROTECTED, IMAPI_FORMAT2_DATA_WRITE_ACTION,
    IMAPI_FORMAT2_DATA_WRITE_ACTION_CALIBRATING_POWER, IMAPI_FORMAT2_DATA_WRITE_ACTION_COMPLETED,
    IMAPI_FORMAT2_DATA_WRITE_ACTION_FINALIZATION, IMAPI_FORMAT2_DATA_WRITE_ACTION_FORMATTING_MEDIA,
    IMAPI_FORMAT2_DATA_WRITE_ACTION_INITIALIZING_HARDWARE,
    IMAPI_FORMAT2_DATA_WRITE_ACTION_VALIDATING_MEDIA, IMAPI_FORMAT2_DATA_WRITE_ACTION_VERIFYING,
    IMAPI_FORMAT2_DATA_WRITE_ACTION_WRITING_DATA, IMAPI_MEDIA_PHYSICAL_TYPE, IMAPI_MEDIA_TYPE_CDRW,
    IMAPI_MEDIA_TYPE_DVDDASHRW, IMAPI_MEDIA_TYPE_DVDPLUSRW, IMAPI_MEDIA_TYPE_DVDPLUSRW_DUALLAYER,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IConnectionPointContainer, IDispatch,
    IDispatch_Impl, IStream, ITypeInfo, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    DISPATCH_FLAGS, DISPPARAMS, EXCEPINFO,
};
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
use windows::Win32::System::Variant::VARIANT;

pub struct ImapiOpticalAdapter;

impl ImapiOpticalAdapter {
    pub fn list_recorders() -> Result<Vec<OpticalRecorder>, DomainError> {
        let _com = ComApartment::new()?;
        unsafe { enumerate_recorders().map_err(|_| DomainError::OpticalBurnFailed) }
    }

    pub fn export_iso(staging: &Path, destination: &Path) -> Result<(), DomainError> {
        validate_staging(staging)?;
        validate_real_directory(destination.parent().ok_or(DomainError::MediaIoFailed)?)?;
        let _com = ComApartment::new()?;
        let stream = unsafe { build_image(staging, None)? };
        copy_stream_to_file(&stream, destination)
    }

    pub fn burn(
        staging: &Path,
        expected_unique_id: &str,
        cancel: Arc<OpticalCancel>,
        progress: OpticalProgressSink,
    ) -> Result<IMAPI_MEDIA_PHYSICAL_TYPE, DomainError> {
        validate_staging(staging)?;
        let _com = ComApartment::new()?;
        let _power = ExecutionStateGuard::new()?;
        unsafe {
            let recorder = recorder_by_id(expected_unique_id)?;
            let format: IDiscFormat2Data =
                CoCreateInstance(&MsftDiscFormat2Data, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| DomainError::OpticalBurnFailed)?;
            format
                .SetRecorder(&recorder)
                .and_then(|()| format.SetClientName(&BSTR::from("MediaDeck")))
                .and_then(|()| format.SetForceMediaToBeClosed(VARIANT_TRUE))
                .map_err(|_| DomainError::OpticalBurnFailed)?;
            if !bool::from(
                format
                    .IsRecorderSupported(&recorder)
                    .map_err(|_| DomainError::OpticalBurnFailed)?,
            ) || !bool::from(
                format
                    .IsCurrentMediaSupported(&recorder)
                    .map_err(|_| DomainError::OpticalMediaNotWritable)?,
            ) {
                return Err(DomainError::OpticalMediaNotWritable);
            }
            let status = format
                .CurrentMediaStatus()
                .map_err(|_| DomainError::OpticalMediaNotWritable)?;
            if status.0 & IMAPI_FORMAT2_DATA_MEDIA_STATE_WRITE_PROTECTED.0 != 0 {
                return Err(DomainError::MediaWriteProtected);
            }
            if status.0 & IMAPI_FORMAT2_DATA_MEDIA_STATE_BLANK.0 == 0 {
                return Err(DomainError::OpticalMediaNotWritable);
            }
            progress(progress_value(
                OpticalPhase::BuildingImage,
                0,
                0,
                None,
                true,
            ));
            let image = build_image(staging, Some(&recorder))?;
            let media_type = format
                .CurrentPhysicalMediaType()
                .map_err(|_| DomainError::OpticalMediaNotWritable)?;
            let sink: DDiscFormat2DataEvents = BurnEvents {
                progress: progress.clone(),
                cancel,
            }
            .into();
            let container: IConnectionPointContainer =
                format.cast().map_err(|_| DomainError::OpticalBurnFailed)?;
            let connection = container
                .FindConnectionPoint(&DDiscFormat2DataEvents::IID)
                .map_err(|_| DomainError::OpticalBurnFailed)?;
            let cookie = connection
                .Advise(&sink)
                .map_err(|_| DomainError::OpticalBurnFailed)?;
            let result = format.Write(&image);
            let _ = connection.Unadvise(cookie);
            result.map_err(|_| DomainError::OpticalBurnFailed)?;
            progress(progress_value(
                OpticalPhase::Completed,
                100,
                0,
                Some(0),
                false,
            ));
            Ok(media_type)
        }
    }

    pub fn erase(expected_unique_id: &str) -> Result<IMAPI_MEDIA_PHYSICAL_TYPE, DomainError> {
        let _com = ComApartment::new()?;
        let _power = ExecutionStateGuard::new()?;
        unsafe {
            let recorder = recorder_by_id(expected_unique_id)?;
            let erase: IDiscFormat2Erase =
                CoCreateInstance(&MsftDiscFormat2Erase, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| DomainError::OpticalBurnFailed)?;
            erase
                .SetRecorder(&recorder)
                .and_then(|()| erase.SetClientName(&BSTR::from("MediaDeck")))
                .and_then(|()| erase.SetFullErase(VARIANT_TRUE))
                .map_err(|_| DomainError::OpticalBurnFailed)?;
            if !bool::from(
                erase
                    .IsRecorderSupported(&recorder)
                    .map_err(|_| DomainError::OpticalBurnFailed)?,
            ) || !bool::from(
                erase
                    .IsCurrentMediaSupported(&recorder)
                    .map_err(|_| DomainError::OpticalMediaNotWritable)?,
            ) {
                return Err(DomainError::OpticalMediaNotWritable);
            }
            let media_type = erase
                .CurrentPhysicalMediaType()
                .map_err(|_| DomainError::OpticalMediaNotWritable)?;
            if !is_rewritable(media_type) {
                return Err(DomainError::OpticalMediaNotWritable);
            }
            erase
                .EraseMedia()
                .map_err(|_| DomainError::OpticalBurnFailed)?;
            Ok(media_type)
        }
    }
}

impl OpticalPort for ImapiOpticalAdapter {
    fn list_recorders(&self) -> Result<Vec<OpticalRecorder>, DomainError> {
        Self::list_recorders()
    }

    fn export_iso(&self, staging: &Path, destination: &Path) -> Result<(), DomainError> {
        Self::export_iso(staging, destination)
    }

    fn burn(
        &self,
        staging: &Path,
        expected_unique_id: &str,
        cancel: Arc<OpticalCancel>,
        progress: OpticalProgressSink,
    ) -> Result<i32, DomainError> {
        Self::burn(staging, expected_unique_id, cancel, progress).map(|kind| kind.0)
    }

    fn erase(&self, expected_unique_id: &str) -> Result<i32, DomainError> {
        Self::erase(expected_unique_id).map(|kind| kind.0)
    }
}

struct ComApartment;

impl ComApartment {
    fn new() -> Result<Self, DomainError> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|_| DomainError::OpticalBurnFailed)?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct ExecutionStateGuard;

impl ExecutionStateGuard {
    fn new() -> Result<Self, DomainError> {
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
        if previous.0 == 0 {
            return Err(DomainError::OpticalBurnFailed);
        }
        Ok(Self)
    }
}

impl Drop for ExecutionStateGuard {
    fn drop(&mut self) {
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

unsafe fn enumerate_recorders() -> windows::core::Result<Vec<OpticalRecorder>> {
    let master: IDiscMaster2 =
        unsafe { CoCreateInstance(&MsftDiscMaster2, None, CLSCTX_INPROC_SERVER)? };
    let mut recorders = Vec::new();
    for index in 0..unsafe { master.Count()? } {
        let id = unsafe { master.get_Item(index)? };
        let recorder: IDiscRecorder2 =
            unsafe { CoCreateInstance(&MsftDiscRecorder2, None, CLSCTX_INPROC_SERVER)? };
        unsafe { recorder.InitializeDiscRecorder(&id)? };
        recorders.push(OpticalRecorder {
            unique_id: id.to_string(),
            vendor: unsafe { recorder.VendorId()? }.to_string(),
            product: unsafe { recorder.ProductId()? }.to_string(),
            volume_paths: unsafe { bstr_safearray(recorder.VolumePathNames()?)? },
        });
    }
    Ok(recorders)
}

unsafe fn recorder_by_id(unique_id: &str) -> Result<IDiscRecorder2, DomainError> {
    let known = unsafe { enumerate_recorders() }.map_err(|_| DomainError::OpticalBurnFailed)?;
    if !known.iter().any(|item| item.unique_id == unique_id) {
        return Err(DomainError::MediaChanged);
    }
    let recorder: IDiscRecorder2 =
        unsafe { CoCreateInstance(&MsftDiscRecorder2, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| DomainError::OpticalBurnFailed)?;
    unsafe { recorder.InitializeDiscRecorder(&BSTR::from(unique_id)) }
        .map_err(|_| DomainError::MediaChanged)?;
    Ok(recorder)
}

unsafe fn bstr_safearray(
    array: *mut windows::Win32::System::Com::SAFEARRAY,
) -> windows::core::Result<Vec<String>> {
    let result = (|| {
        let lower = unsafe { SafeArrayGetLBound(array, 1)? };
        let upper = unsafe { SafeArrayGetUBound(array, 1)? };
        let mut values = Vec::new();
        for index in lower..=upper {
            let mut raw: *mut u16 = std::ptr::null_mut();
            unsafe { SafeArrayGetElement(array, &index, (&mut raw as *mut *mut u16).cast())? };
            if !raw.is_null() {
                let value = unsafe { BSTR::from_raw(raw) };
                values.push(value.to_string());
            }
        }
        Ok(values)
    })();
    let _ = unsafe { SafeArrayDestroy(array) };
    result
}

unsafe fn build_image(
    staging: &Path,
    recorder: Option<&IDiscRecorder2>,
) -> Result<IStream, DomainError> {
    let image: IFileSystemImage =
        unsafe { CoCreateInstance(&MsftFileSystemImage, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| DomainError::OpticalBurnFailed)?;
    if let Some(recorder) = recorder {
        unsafe { image.ChooseImageDefaults(recorder) }
            .map_err(|_| DomainError::OpticalBurnFailed)?;
    } else {
        unsafe {
            image
                .SetFileSystemsToCreate(FsiFileSystems(
                    FsiFileSystemISO9660.0 | FsiFileSystemJoliet.0,
                ))
                .and_then(|()| image.SetISO9660InterchangeLevel(1))
        }
        .map_err(|_| DomainError::OpticalBurnFailed)?;
    }
    unsafe {
        image
            .SetVolumeName(&BSTR::from("MEDIADECK"))
            .and_then(|()| image.SetStrictFileSystemCompliance(VARIANT_TRUE))
    }
    .map_err(|_| DomainError::OpticalBurnFailed)?;
    let root = unsafe { image.Root() }.map_err(|_| DomainError::OpticalBurnFailed)?;
    let source = staging.to_str().ok_or(DomainError::MediaIoFailed)?;
    unsafe { root.AddTree(&BSTR::from(source), VARIANT_FALSE) }
        .map_err(|_| DomainError::OpticalBurnFailed)?;
    let result =
        unsafe { image.CreateResultImage() }.map_err(|_| DomainError::OpticalBurnFailed)?;
    unsafe { result.ImageStream() }.map_err(|_| DomainError::OpticalBurnFailed)
}

fn copy_stream_to_file(stream: &IStream, destination: &Path) -> Result<(), DomainError> {
    if destination.exists() {
        return Err(DomainError::MediaIoFailed);
    }
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|_| DomainError::MediaIoFailed)?;
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let mut read = 0_u32;
        unsafe {
            stream.Read(
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer.len() as u32,
                Some(&mut read),
            )
        }
        .ok()
        .map_err(|_| DomainError::MediaIoFailed)?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read as usize])
            .map_err(|_| DomainError::MediaIoFailed)?;
    }
    output.sync_all().map_err(|_| DomainError::MediaIoFailed)
}

fn validate_staging(path: &Path) -> Result<(), DomainError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DomainError::MediaIoFailed)?;
    let profile = fs::symlink_metadata(path.join("GAME.INI"))
        .map_err(|_| DomainError::MediaProfileMissing)?;
    if !metadata.is_dir()
        || is_reparse_point(&metadata)
        || !profile.is_file()
        || is_reparse_point(&profile)
    {
        return Err(DomainError::MediaIoFailed);
    }
    Ok(())
}

fn validate_real_directory(path: &Path) -> Result<(), DomainError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DomainError::MediaIoFailed)?;
    if metadata.is_dir() && !is_reparse_point(&metadata) {
        Ok(())
    } else {
        Err(DomainError::MediaIoFailed)
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn is_rewritable(media_type: IMAPI_MEDIA_PHYSICAL_TYPE) -> bool {
    matches!(
        media_type,
        IMAPI_MEDIA_TYPE_CDRW
            | IMAPI_MEDIA_TYPE_DVDDASHRW
            | IMAPI_MEDIA_TYPE_DVDPLUSRW
            | IMAPI_MEDIA_TYPE_DVDPLUSRW_DUALLAYER
    )
}

#[implement(DDiscFormat2DataEvents)]
struct BurnEvents {
    progress: Arc<dyn Fn(OpticalProgress) + Send + Sync>,
    cancel: Arc<OpticalCancel>,
}

impl IDispatch_Impl for BurnEvents_Impl {
    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
        Ok(0)
    }

    fn GetTypeInfo(&self, _index: u32, _lcid: u32) -> windows::core::Result<ITypeInfo> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }

    fn GetIDsOfNames(
        &self,
        _riid: *const windows::core::GUID,
        _names: *const windows::core::PCWSTR,
        _count: u32,
        _lcid: u32,
        _ids: *mut i32,
    ) -> windows::core::Result<()> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }

    fn Invoke(
        &self,
        _member: i32,
        _riid: *const windows::core::GUID,
        _lcid: u32,
        _flags: DISPATCH_FLAGS,
        _parameters: *const DISPPARAMS,
        _result: *mut VARIANT,
        _exception: *mut EXCEPINFO,
        _argument_error: *mut u32,
    ) -> windows::core::Result<()> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }
}

impl DDiscFormat2DataEvents_Impl for BurnEvents_Impl {
    fn Update(
        &self,
        object: Ref<'_, IDispatch>,
        event: Ref<'_, IDispatch>,
    ) -> windows::core::Result<()> {
        let event = event
            .clone()
            .ok_or_else(|| windows::core::Error::from_hresult(E_NOTIMPL))?;
        let args: IDiscFormat2DataEventArgs = event.cast()?;
        let action = unsafe { args.CurrentAction()? };
        let elapsed = unsafe { args.ElapsedTime()? }.max(0) as u32;
        let total = unsafe { args.TotalTime()? }.max(0) as u32;
        let remaining = unsafe { args.RemainingTime()? }.max(0) as u32;
        let safe = cancellation_safe(action);
        let percent = elapsed
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(0)
            .min(100) as u8;
        (self.progress)(progress_value(
            phase(action),
            percent,
            elapsed,
            Some(remaining),
            safe,
        ));
        if safe && self.cancel.requested() {
            let object = object
                .clone()
                .ok_or_else(|| windows::core::Error::from_hresult(E_NOTIMPL))?;
            let format: IDiscFormat2Data = object.cast()?;
            unsafe { format.CancelWrite()? };
        }
        Ok(())
    }
}

fn phase(action: IMAPI_FORMAT2_DATA_WRITE_ACTION) -> OpticalPhase {
    match action {
        IMAPI_FORMAT2_DATA_WRITE_ACTION_VALIDATING_MEDIA => OpticalPhase::Validating,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_FORMATTING_MEDIA => OpticalPhase::FormattingMedia,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_INITIALIZING_HARDWARE => OpticalPhase::InitializingHardware,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_CALIBRATING_POWER => OpticalPhase::CalibratingPower,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_WRITING_DATA => OpticalPhase::WritingData,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_FINALIZATION => OpticalPhase::Finalizing,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_VERIFYING => OpticalPhase::Verifying,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_COMPLETED => OpticalPhase::Completed,
        _ => OpticalPhase::Validating,
    }
}

fn cancellation_safe(action: IMAPI_FORMAT2_DATA_WRITE_ACTION) -> bool {
    matches!(
        action,
        IMAPI_FORMAT2_DATA_WRITE_ACTION_VALIDATING_MEDIA
            | IMAPI_FORMAT2_DATA_WRITE_ACTION_INITIALIZING_HARDWARE
            | IMAPI_FORMAT2_DATA_WRITE_ACTION_CALIBRATING_POWER
            | IMAPI_FORMAT2_DATA_WRITE_ACTION_WRITING_DATA
    )
}

fn progress_value(
    phase: OpticalPhase,
    percent: u8,
    elapsed_seconds: u32,
    remaining_seconds: Option<u32>,
    cancellation_safe: bool,
) -> OpticalProgress {
    OpticalProgress {
        phase,
        percent,
        elapsed_seconds,
        remaining_seconds,
        cancellation_safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cancellation_is_never_exposed_during_finalization() {
        assert!(cancellation_safe(
            IMAPI_FORMAT2_DATA_WRITE_ACTION_WRITING_DATA
        ));
        assert!(!cancellation_safe(
            IMAPI_FORMAT2_DATA_WRITE_ACTION_FINALIZATION
        ));
        assert!(!cancellation_safe(
            IMAPI_FORMAT2_DATA_WRITE_ACTION_COMPLETED
        ));
    }

    #[test]
    fn erase_accepts_only_known_rw_media_types() {
        assert!(is_rewritable(IMAPI_MEDIA_TYPE_CDRW));
        assert!(is_rewritable(IMAPI_MEDIA_TYPE_DVDDASHRW));
        assert!(!is_rewritable(
            windows::Win32::Storage::Imapi::IMAPI_MEDIA_TYPE_CDR
        ));
    }

    #[test]
    fn exports_mountable_iso9660_image_without_hardware() {
        let directory = tempdir().unwrap();
        let staging = directory.path().join("staging");
        fs::create_dir(&staging).unwrap();
        fs::write(
            staging.join("GAME.INI"),
            b"[MEDIA]\r\nSCHEMA=2\r\nMEDIA_ID=FIXTURE\r\n",
        )
        .unwrap();
        let iso = directory.path().join("fixture.iso");
        ImapiOpticalAdapter::export_iso(&staging, &iso).unwrap();
        let bytes = fs::read(iso).unwrap();
        assert!(bytes.len() >= 17 * 2048);
        assert_eq!(&bytes[16 * 2048 + 1..16 * 2048 + 6], b"CD001");
        assert_eq!(bytes.len() % 2048, 0);
    }

    #[test]
    #[ignore = "HIL: requires an explicitly selected physical IMAPI recorder and blank media"]
    fn hil_enumerates_real_recorder() {
        assert!(!ImapiOpticalAdapter::list_recorders().unwrap().is_empty());
    }
}
