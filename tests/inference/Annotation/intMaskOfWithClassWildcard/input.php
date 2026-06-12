<?php
class FileFlag {
    public const OPEN = 1;
    public const MODIFIED = 2;
    public const NEW = 4;
}

/**
 * @param int-mask-of<FileFlag::*> $flags
 */
function takesFlags(int $flags) : void {
    echo $flags;
}

takesFlags(FileFlag::MODIFIED | FileFlag::NEW);
