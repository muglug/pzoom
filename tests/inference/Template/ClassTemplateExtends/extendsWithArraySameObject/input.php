<?php
/**
 * @template Tv
 */
interface C1 {
    /**
     * @return C1<array<int, Tv>>
     */
    public function zip(): C1;
}

/**
 * @template Tv
 * @extends C1<Tv>
 */
interface C2 extends C1 {
    /**
     * @return C2<array<int, Tv>>
     */
    public function zip(): C2;
}