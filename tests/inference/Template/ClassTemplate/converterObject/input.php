<?php
/**
 * @template I as array-key
 * @template V
 */
class Converter
{
    /**
     * @var array<I, V> $records
     */
    public $records;

    /**
     * @param array<I, V> $records
     */
    public function __construct(array $records) {
        $this->records = $records;
    }

    /**
     * @template Q2 as object
     *
     * @param Q2 $obj2
     *
     * @return array<I, V|Q2>
     */
    private function appender(object $obj2): array
    {
        $arr = [];
        foreach ($this->records as $key => $obj) {
            if (rand(0, 1)) {
                $obj = $obj2;
            }
            $arr[$key] = $obj;
        }

        return $arr;
    }

    /**
     * @template Q1 as object
     *
     * @param Q1 $obj
     *
     * @return array<I, V|Q1>
     */
    public function appendProperty(object $obj): array
    {
        return $this->appender($obj);
    }
}