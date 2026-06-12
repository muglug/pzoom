<?php
/**
 * @psalm-suppress MixedMethodCall
 */
function foo(string $s, object $o) : void {
    $o->foo("COUNT{$s}");
}
