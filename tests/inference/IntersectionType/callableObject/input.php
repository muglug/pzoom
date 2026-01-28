<?php
/**
 * @param object&callable():void $object
 */
function takesCallableObject(object $object): void {
    $object();
}
