<?php
/** @param non-falsy-string|null $string_value */
function f(?string $string_value): void {
    if ($string_value && class_exists($string_value)) {
        $r = new \ReflectionClass($string_value);
        echo $r->getName();
    }
}
