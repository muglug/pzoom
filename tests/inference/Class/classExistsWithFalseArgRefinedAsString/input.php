<?php
/**
 * @param class-string $class
 * @return string
 */
function autoload(string $class) : string {
    if (class_exists($class, false)) {
        return $class;
    }

    return $class;
}
