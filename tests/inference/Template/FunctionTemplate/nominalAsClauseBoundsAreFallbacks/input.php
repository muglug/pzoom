<?php

/** @param callable(string):bool $filter */
function getFilesInDir191(string $dir_path, callable $filter): void
{
    $iterator = new RecursiveDirectoryIterator(
        $dir_path,
        FilesystemIterator::CURRENT_AS_PATHNAME | FilesystemIterator::SKIP_DOTS,
    );

    $iterator = new RecursiveCallbackFilterIterator(
        $iterator,
        /** @param mixed $_ */
        static function (string $current, mixed $_, RecursiveIterator $iterator) use ($filter): bool {
            if ($iterator->hasChildren()) {
                $path = $current . DIRECTORY_SEPARATOR;
            } else {
                $path = $current;
            }

            return $filter($path);
        },
    );

    echo get_class($iterator);
}
