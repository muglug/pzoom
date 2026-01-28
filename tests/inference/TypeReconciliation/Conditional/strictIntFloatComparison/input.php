<?php
/**
 * @psalm-suppress InvalidReturnType
 * @psalm-suppress MismatchingDocblockReturnType
 * @return ($bar is int ? list<int> : list<float>)
 */
function foo($bar): string {}

/** @var int */
$baz = 1;
$a = foo($baz);

/** @var float */
$baz = 1.;
$b = foo($baz);

/** @var int|float */
$baz = 1;
$c = foo($baz);
                