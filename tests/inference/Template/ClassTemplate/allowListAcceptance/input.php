<?php
/** @template T */
class Collection
{
    /** @var list<T> */
    public $values;

    /** @param list<T> $values */
    function __construct(array $values)
    {
        $this->values = $values;
    }
}

/** @return Collection<string> */
function makeStringCollection()
{
    return new Collection(getStringList()); // gets typed as Collection<mixed> for some reason
}

/** @return list<string> */
function getStringList(): array
{
    return ["foo", "baz"];
}