<?php

class AtomicType {}

class TypeAssertion {
    public ?AtomicType $inner = null;

    /** @psalm-mutation-free */
    public function getAtomicType(): ?AtomicType
    {
        return $this->inner;
    }
}

class TypeUnion {
    /** @param non-empty-list<AtomicType> $types */
    public function __construct(public array $types) {}
}

function buildUnion(TypeAssertion $assertion): ?TypeUnion
{
    if ($assertion->getAtomicType()) {
        return new TypeUnion([$assertion->getAtomicType()]);
    }
    return null;
}
