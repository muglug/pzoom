<?php
/**
 */
function foo(string $s, object $o) : void {
    $o->foo("COUNT{$s}");
}
