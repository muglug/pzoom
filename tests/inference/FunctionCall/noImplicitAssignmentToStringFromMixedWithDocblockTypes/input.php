<?php
/** @param string $s */
function takesString($s) : void {}
function takesInt(int $i) : void {}

/**
 * @param mixed $s
 * @psalm-suppress MixedArgument
 */
function bar($s) : void {
    takesString($s);
    takesInt($s);
}
