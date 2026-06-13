<?php
$a1 = str_starts_with(uniqid(), "");
/** @psalm-suppress InvalidLiteralArgument */
$b1 = str_starts_with("", "random string");
$c1 = str_starts_with(uniqid(), "random string");

$a2 = str_ends_with(uniqid(), "");
/** @psalm-suppress InvalidLiteralArgument */
$b2 = str_ends_with("", "random string");
$c2 = str_ends_with(uniqid(), "random string");

$a3 = str_contains(uniqid(), "");
/** @psalm-suppress InvalidLiteralArgument */
$b3 = str_contains("", "random string");
$c3 = str_contains(uniqid(), "random string");
