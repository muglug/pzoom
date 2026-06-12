<?php
class S {
    protected array $a = [];
    protected array $b = [];
    protected array $c = [];

    /**
     * @psalm-suppress MixedAssignment
     */
    public function pop(): void {
        if (!$this->a) {
            return;
        }

        $popped = array_pop($this->a);

        /** @psalm-suppress MixedArrayAccess */
        [$this->b, $this->c] = $popped;
    }
}
