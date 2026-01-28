<?php
/**
 * @param class-string<object&callable(string):void> $className
 */
function takesCallableObject(string $className): void {
    $object = new $className();
    $object("foo");
}
