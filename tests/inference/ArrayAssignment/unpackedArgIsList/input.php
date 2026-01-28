<?php
final class Values
{
    /**
     * @psalm-var list<int>
     */
    private $ints = [];

    /** @no-named-arguments */
    public function set(int ...$ints): void {
        $this->ints = $ints;
    }
}
