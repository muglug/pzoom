<?php
/** @psalm-suppress InvalidReturnType */
function foo(): array {}

$b = foo();
if (!empty($b["hello"]) && $b["hello"] !== "off") {
    $a = true;
} else {
    $a = false;
}
