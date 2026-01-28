<?php
class A {
    const FOO = "foo";
    const BAR = "bar";
    const BAT = "bat";
    const BAM = "bam";

    /** @var self::FOO|self::BAR|self::BAT|null $s */
    public $s;

    public function isFooOrBar() : void {
        $map = [
            A::FOO => 1,
            A::BAR => 1,
            A::BAM => 1,
        ];

        if ($this->s !== null && isset($map[$this->s])) {}
    }
}