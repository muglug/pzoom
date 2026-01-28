<?php
function a(string $_b): void {}

function foo(?string $c): string {
    $iftrue = $c !== null;

    if ($c !== null) {
        a($c);
    }

    if ($iftrue) {
        return $c;
    }

    return "";
}