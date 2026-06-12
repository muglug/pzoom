<?php
function takesNullableString(?string $s) : void {}
function takesString(string $s) : void {}

/**
 * @param mixed $s
 * @psalm-suppress MixedArgument
 */
function bar($s) : void {
    takesNullableString($s);
    takesString($s);
}
