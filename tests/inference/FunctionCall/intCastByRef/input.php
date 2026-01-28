<?php
function foo(int &$i) : void {}

$a = rand(0, 1) ? null : 5;
/** @psalm-suppress MixedArgument */
foo((int) $a);
