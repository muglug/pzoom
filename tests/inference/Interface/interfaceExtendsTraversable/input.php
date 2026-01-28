<?php
/**
 * @extends IteratorAggregate<mixed, mixed>
 * @extends ArrayAccess<mixed, mixed>
 */
interface Collection extends Countable, IteratorAggregate, ArrayAccess {}

function takesCollection(Collection $c): void {
    takesIterable($c);
}

function takesIterable(iterable $i): void {}
