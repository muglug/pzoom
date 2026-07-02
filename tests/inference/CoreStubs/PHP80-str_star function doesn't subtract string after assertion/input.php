<?php
/** @return false|string */
function after_str_contains()
{
    $string = file_get_contents("");
    if (!str_contains($string, "foo")) {
        return $string;
    }
    throw new RuntimeException();
}

/** @return false|string */
function after_str_starts_with()
{
    $string = file_get_contents("");
    if (!str_starts_with($string, "foo")) {
        return $string;
    }
    throw new RuntimeException();
}

/** @return false|string */
function after_str_ends_with()
{
    $string = file_get_contents("");
    if (!str_ends_with($string, "foo")) {
        return $string;
    }
    throw new RuntimeException();
}
$a = after_str_contains();
$b = after_str_starts_with();
$c = after_str_ends_with();
