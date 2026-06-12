<?php
/**
 * @param Closure(string ...):void $fn
 */
function test(Closure $fn): void {
    $fn("a", "b");
}

test(function(string ...$args): void {});
