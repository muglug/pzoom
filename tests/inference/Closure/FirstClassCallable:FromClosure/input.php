<?php
$closure = fn (string $string): int => strlen($string);
$closure = $closure(...);
