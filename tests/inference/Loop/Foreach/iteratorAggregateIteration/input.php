<?php
/**
 * @psalm-suppress MissingTemplateParam
 */
class C implements IteratorAggregate
{
    public function getIterator(): Iterator
    {
        return new ArrayIterator([]);
    }
}

function loopT(Traversable $coll): void
{
    foreach ($coll as $item) {}
}

function loopI(IteratorAggregate $coll): void
{
    foreach ($coll as $item) {}
}

loopT(new C);
loopI(new C);
