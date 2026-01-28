<?php
namespace Foo;

/**
 * @template T
 */
final class Set
{
    /** @var \Iterator<T> */
    private \Iterator $values;

    /**
     * @param \Iterator<T> $values
     */
    public function __construct(\Iterator $values)
    {
        $this->values = $values;
    }

    /**
     * @param T $element
     *
     * @return self<T>
     */
    public function __invoke($element): self
    {
        return new self(
            (
                function($values, $element): \Generator {
                    /** @var T $value */
                    foreach ($values as $value) {
                        yield $value;
                    }

                    yield $element;
                }
            )($this->values, $element),
        );
    }
}