<?php
/**
 * @psalm-assert "foo"|"bar"|"foo-bar" $s
 */
function assertFooBar(string $s) : void {
}

$a = "";
assertFooBar($a);
