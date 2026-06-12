<?php
function foo(?string $s): string {
    $s ??= "Hello";
    return $s;
}
