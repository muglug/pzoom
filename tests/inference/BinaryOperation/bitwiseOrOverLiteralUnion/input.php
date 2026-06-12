<?php
class Attr2 {
    public const TARGET_CLASS = 1;
    public const TARGET_FUNCTION = 2;
    public const TARGET_METHOD = 4;
    public const TARGET_PROPERTY = 8;
    public const TARGET_CLASS_CONSTANT = 16;
    public const TARGET_PARAMETER = 32;
}

/** @param int-mask<1, 2, 4, 8, 16, 32> $_target */
function takesTarget(int $_target): void {}

function f(bool $promoted): void {
    takesTarget(Attr2::TARGET_PARAMETER | ($promoted ? Attr2::TARGET_PROPERTY : 0));
}
