<?php

namespace Rabus\PsalmReturnTypeWillChange;

use EmptyIterator;
use IteratorAggregate;
use ReturnTypeWillChange;

/**
 * @psalm-suppress MissingTemplateParam
 */
final class EmptyCollection implements IteratorAggregate
{
    #[ReturnTypeWillChange]
    public function getIterator()
    {
        return new EmptyIterator();
    }
}
