<?php
class Bar {
    /** @var array{0: string, 1:string} */
    private array $baz = ["a", "b"];

    public function append(string $str) : void {
        $this->baz[rand(0, 1) ? 0 : 1] = $str;
    }
}
