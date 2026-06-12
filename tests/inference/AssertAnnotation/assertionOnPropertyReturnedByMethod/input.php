<?php
class a {
    public ?int $id = null;
    /**
     * @psalm-mutation-free
     *
     * @psalm-assert-if-true !null $this->id
     */
    public function isExists(): bool {
        return $this->id !== null;
    }
}

class b {
    public ?int $id = null;
    public function __construct(private a $a) {
        if ($this->getA()->isExists()) {
            /** @psalm-check-type-exact $this->id = ?int */
        }
    }
    public function getA(): a { return $this->a; }
}
