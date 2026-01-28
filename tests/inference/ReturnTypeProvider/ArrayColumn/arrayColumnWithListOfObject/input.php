<?php
function foo(object $object): void {}

/** @var list<object> $instances */
$instances = [];
foreach (array_column($instances, null, "name") as $instance) {
    foo($instance);
}
            
