<?php
/**
 * @psalm-suppress MissingConstructor
 */
class A {
    public string $bar;

    public function getBar() : string {
        /** @psalm-suppress RedundantPropertyInitializationCheck */
        if (!isset($this->bar)) {
            return "hello";
        }

        return $this->bar;
    }

    public function getBarAgain() : string {
        /** @psalm-suppress RedundantPropertyInitializationCheck */
        if (isset($this->bar)) {
            return $this->bar;
        }

        return "hello";
    }
}
