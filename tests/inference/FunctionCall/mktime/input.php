<?php
/** @psalm-suppress InvalidScalarArgument */
$a = mktime("foo");
/** @psalm-suppress MixedArgument */
$b = mktime($GLOBALS["foo"]);
$c = mktime(1, 2, 3);
