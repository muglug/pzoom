<?php
function foo(): bool { return false; }
$match = match (foo()) {
    false => null,
    true => 1,
} ?? 2;
