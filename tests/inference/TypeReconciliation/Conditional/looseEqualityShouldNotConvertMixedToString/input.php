<?php
/** @var mixed */
$int = 0;
/** @var string */
$string = "0";

function takes_string(string $string) : void {}
function takes_int(int $int) : void {}

if ($int == $string) {
    /** @psalm-suppress MixedArgument */
    takes_int($int);
}
