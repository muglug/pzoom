<?php
interface I {}

function takesArray(array $_a): void {}

/** @param string|I|string[]|I[] $p */
function eliminatesNonArray($p): void {
    if (is_array($p)) {
        takesArray($p);
    }
}