<?php
class A {
    /**
     * @return array<int, self|null>
     */
    public function getRows() : array {
        return [new self, null];
    }

    public function filter() : void {
        $arr = array_filter(
            static::getRows(),
            function (self $row) : bool {
                return is_a($row, static::class);
            }
        );
    }
}
