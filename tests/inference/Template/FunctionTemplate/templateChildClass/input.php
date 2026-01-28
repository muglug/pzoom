<?php
/** @template T */
class Collection {
    /**
     * @param T $t
     */
    private function add($t) : void {}

    /**
     * @template TChild as T
     * @param TChild $default
     *
     * @return TChild
     */
    public function get($default)
    {
        $this->add($default);

        return $default;
    }
}