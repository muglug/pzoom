<?php
/**
 * @param class-string<object&callable():void> $className
 */
function takesCallableObject(string $className): void {
    $object = new $className();
    $object();
}
