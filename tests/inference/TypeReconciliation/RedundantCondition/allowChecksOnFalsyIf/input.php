<?php
function foo(?string $s) : string {
    if ($s == null) {
        if ($s === null) {}

        return "hello";
    } else {
        return $s;
    }
}