<?php
function contains(string $a, string $b, mixed $element): void
{
    if (in_array($element, [$a], true)) {
    } elseif (in_array($element, [$b], true)) {
    }
}