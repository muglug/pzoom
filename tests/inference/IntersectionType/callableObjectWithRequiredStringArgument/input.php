<?php
/**
 * @param object&callable(string):void $object
 */
function takesCallableObject(object $object): void {
    $object("foo");
}
