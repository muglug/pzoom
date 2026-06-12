<?php
function takesInt(int $i): void {}
function takesString(string $s): void {}
function takesBool(bool $b): void {}

/**
 * @param array{int, string, bool} $b
 */
function a(array $b): void {
    takesInt($b[0]);
    takesString($b[1]);
    takesBool($b[2]);
}
