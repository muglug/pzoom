<?php
function foo(iterable $iterable) : void {
    if (\is_array($iterable)) {}
    if ($iterable instanceof \Traversable) {}
}