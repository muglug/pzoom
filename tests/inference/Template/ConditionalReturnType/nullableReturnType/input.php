<?php
/**
 * @psalm-return ($name is "foo" ? string : null)
 */
function get(string $name) : ?string {
    if ($name === "foo") {
        return "hello";
    }
    return null;
}