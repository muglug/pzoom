<?php
class ImageFile {}
class MusicFile {}

/**
 * @template T of object
 */
interface FileManager {
    /**
     * @param class-string<T> $instance
     * @return T
     */
    public function create(string $instance) : object;
}

/** @param FileManager<ImageFile> $m */
function foo(FileManager $m) : void {
    $m->create(MusicFile::class);
}
