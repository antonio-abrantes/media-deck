ALTER TABLE launch_profiles
ADD COLUMN steam_launch_option INTEGER
CHECK (steam_launch_option IS NULL OR steam_launch_option BETWEEN 0 AND 31);
