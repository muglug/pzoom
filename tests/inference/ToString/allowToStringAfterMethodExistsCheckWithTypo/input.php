<?php
function getString(object $value) : ?string {
    if (method_exists($value, "__toStrong")) {
        return (string) $value;
    }

    return null;
}
