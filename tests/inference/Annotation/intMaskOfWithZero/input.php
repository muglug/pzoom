<?php
class FileFlag {
    public const OPEN = 1;
    public const MODIFIED = 2;
    public const NEW = 4;
}

/** @param int-mask-of<FileFlag::*> $_flags */
function takesFlags(int $_flags): void {}

takesFlags(0);
