<?php
function foo(iterable $iterable) : void {
    if (\is_array($iterable) && $iterable instanceof \Traversable) {}
}
