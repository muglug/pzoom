<?php
$a = microtime(true);
$b = microtime();
/** @psalm-suppress InvalidScalarArgument */
$c = microtime(1);
$d = microtime(false);
