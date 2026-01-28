<?php
/**
 * @extends \IteratorAggregate<int, string>
 */
interface IStringList extends \IteratorAggregate
{
    /** @return \Iterator<int, string> */
    public function getIterator(): \Iterator;
}

class StringListDecorator implements IStringList
{
    private IStringList $decorated;

    public function __construct(IStringList $decorated) {
        $this->decorated = $decorated;
    }

    public function getIterator(): \Iterator
    {
        yield from $this->decorated;
    }
}