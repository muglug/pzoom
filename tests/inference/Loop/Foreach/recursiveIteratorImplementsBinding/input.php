<?php
/** @implements RecursiveIterator<string, DateTime> */
class B implements RecursiveIterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    /** @return DateTime */
    public function current(): DateTime { return new DateTime(); }
    public function key(): string { return ''; }
    public function next(): void {}
    public function hasChildren(): bool { return false; }
    public function getChildren(): null|self { return null; }
}
function g(B $b): void {
    foreach ($b as $k => $y) {
        echo $k;
        echo $y->format('Y');
    }
}
