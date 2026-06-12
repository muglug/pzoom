<?php
function test(string $s): void {
    if (!$s || strlen($s) !== 9) {
        throw new Exception();
    }
}
