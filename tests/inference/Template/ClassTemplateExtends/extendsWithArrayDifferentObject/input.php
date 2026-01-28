<?php
/**
 * @template Tv
 */
interface C1 {
    /**
     * @return D1<array<int, Tv>>
     */
    public function zip(): D1;
}

/**
 * @template Tv
 * @extends C1<Tv>
 */
interface C2 extends C1 {
    /**
     * @return D2<array<int, Tv>>
     */
    public function zip(): D2;
}

/**
 * @template Tv
 */
interface D1 {}

/**
 * @template Tv
 * @extends D1<Tv>
 */
interface D2 extends D1 {}