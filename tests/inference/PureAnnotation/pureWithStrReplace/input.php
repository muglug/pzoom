<?php
/** @psalm-pure */
function highlight(string $needle, string $output) : string {
    $needle = preg_quote($needle, '#');
    $needles = str_replace(['"', ' '], ['', '|'], $needle);
    $output = (string) preg_replace("#({$needles})#im", "<mark>$1</mark>", $output);

    return $output;
}
