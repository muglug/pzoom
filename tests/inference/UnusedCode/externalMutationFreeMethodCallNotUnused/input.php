<?php
/** @psalm-immutable */
final class A {
    private ?int $cached = null;
    /**
     * @psalm-external-mutation-free
     * @psalm-suppress InaccessibleProperty
     */
    private function compute(): void {
        $this->cached = 5;
    }
    public function getCached(): ?int {
        $this->compute();
        return $this->cached;
    }
}
echo (new A)->getCached();
