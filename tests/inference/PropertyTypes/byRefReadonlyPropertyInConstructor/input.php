<?php

/** @psalm-immutable */
class SortedShape {
    /** @var array<string, int> */
    public array $properties;

    /** @param array<string, int> $properties */
    public function __construct(array $properties)
    {
        $this->properties = $properties;
        ksort($this->properties);
    }

    public function mutateOutsideConstructor(): void
    {
        /** @psalm-suppress ImpureFunctionCall */
        ksort($this->properties);
    }
}
