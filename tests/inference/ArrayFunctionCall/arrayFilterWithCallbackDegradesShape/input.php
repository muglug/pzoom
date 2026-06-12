<?php
class Composer9 {
    public static function getLockFilePath(string $root): string {
        return $root . '/composer.lock';
    }
}

/** @return non-empty-array<int,string> */
function lockFiles9(string $psalm_root, string $project_root): array {
    if ($psalm_root === $project_root) {
        $composer_lock_filenames = [
            Composer9::getLockFilePath($psalm_root),
        ];
    } else {
        $composer_lock_filenames = [
            Composer9::getLockFilePath($project_root),
            Composer9::getLockFilePath($psalm_root . '/../../..'),
            Composer9::getLockFilePath($psalm_root),
        ];
    }

    $composer_lock_filenames = array_filter($composer_lock_filenames, 'is_readable');

    if (empty($composer_lock_filenames)) {
        $composer_lock_filenames[] = 'stub';
    }

    return $composer_lock_filenames;
}
