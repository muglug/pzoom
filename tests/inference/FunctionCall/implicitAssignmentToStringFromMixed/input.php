<?php
/** @param "a"|"b" $s */
function takesString(string $s) : void {}
function takesInt(int $i) : void {}

/**
 * @param mixed $s
 * @psalm-suppress MixedArgument
 */
function bar($s) : void {
    takesString($s);
    takesInt($s);
}
