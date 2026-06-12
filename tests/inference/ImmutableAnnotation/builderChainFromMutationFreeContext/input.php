<?php
class BuilderW {
    /** @var list<string> */
    public array $types = [];

    /**
     * @psalm-external-mutation-free
     * @param list<string> $types
     */
    public function setTypes(array $types): self {
        $this->types = $types;
        return $this;
    }

    /** @psalm-mutation-free */
    public function freeze(): FrozenW { return new FrozenW(); }
}
class FrozenW {
    /** @psalm-mutation-free */
    public function getBuilder(): BuilderW { return new BuilderW(); }

    /**
     * @psalm-mutation-free
     * @param list<string> $types
     */
    public function setTypes2(array $types): FrozenW {
        return $this->getBuilder()->setTypes($types)->freeze();
    }
}
