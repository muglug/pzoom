<?php
/** @psalm-suppress InvalidReturnType */
function foo(): array {}

$b = foo();
$a = !empty($b["hello"]) && $b["hello"] !== "off" ? true : false;
