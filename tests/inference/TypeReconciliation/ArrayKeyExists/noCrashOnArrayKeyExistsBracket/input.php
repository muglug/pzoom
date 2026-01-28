<?php
class MyCollection {
    /**
     * @param int $commenter
     * @param int $numToGet
     * @return int[]
     */
    public function getPosters($commenter, $numToGet=10) {
        $posters = array();
        $count = 0;
        $a = new ArrayObject([[1234]]);
        $iter = $a->getIterator();
        while ($iter->valid() && $count < $numToGet) {
            $value = $iter->current();
            if ($value[0] != $commenter) {
                if (!array_key_exists($value[0], $posters)) {
                    $posters[$value[0]] = 1;
                    $count++;
                }
            }
            $iter->next();
        }
        return array_keys($posters);
    }
}
