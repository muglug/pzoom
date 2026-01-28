<?php
class MyTestCase {
    /** @return iterable<int,array<int,int>> */
    public function provide() {
        yield [1];
    }
}