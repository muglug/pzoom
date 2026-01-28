<?php
/** @param class-string $className */
$className = stdClass::class;
if (class_exists($className)) {
    throw new \RuntimeException($className);
}
