<?php
final class TT {}
final class MU {
    /** @var array<string, TT> */
    private array $types = [];

    public function hasMixed(): bool {
        return $this->types !== [];
    }

    /**
     * @psalm-external-mutation-free
     */
    public function substitute(MU $old): self {
        if ($this->hasMixed()) {
            return $this;
        }
        foreach ($old->types as $k => $_v) {
            if (!isset($this->types[$k])) {
                $this->types[$k] = new TT();
            }
        }
        return $this;
    }
}
(new MU())->substitute(new MU());
