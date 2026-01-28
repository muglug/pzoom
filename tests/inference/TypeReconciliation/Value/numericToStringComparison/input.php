<?php
/** @psalm-suppress MissingParamType */
function foo($s) : void {
    if (is_numeric($s)) {
        if ($s === 1) {}
    }
}