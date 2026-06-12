<?php
/** @return non-empty-string */
function after_str_contains(): string
{
    $string = file_get_contents("");
    if (str_contains($string, "foo")) {
        return $string;
    }
    throw new RuntimeException();
}

/** @return non-empty-string */
function after_str_starts_with(): string
{
    $string = file_get_contents("");
    if (str_starts_with($string, "foo")) {
        return $string;
    }
    throw new RuntimeException();
}

/** @return non-empty-string */
function after_str_ends_with(): string
{
    $string = file_get_contents("");
    if (str_ends_with($string, "foo")) {
        return $string;
    }
    throw new RuntimeException();
}

/** @return non-empty-string */
function after_strpos(): string
{
    $string = uniqid();
    if (strpos($string, "foo") !== false) {
        return $string;
    }
    throw new RuntimeException();
}

/** @return non-empty-string */
function after_stripos(): string
{
    $string = uniqid();
    if (stripos($string, "foo") !== false) {
        return $string;
    }
    throw new RuntimeException();
}

$a = after_str_contains();
$b = after_str_starts_with();
$c = after_str_ends_with();
$d = after_strpos();
$e = after_stripos();
