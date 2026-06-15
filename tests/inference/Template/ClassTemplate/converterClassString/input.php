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
     * @template Q as object
     *
     * @param class-string<Q> $obj
     *
     * @return array<I, V|Q>
     */
    public function appendProperty(string $obj): array
    {
        return $this->appender($obj);
    }

    /**
     * @template Q as object
     *
     * @param class-string<Q> $obj2
     *
     * @return array<I, V|Q>
     *
     */
    private function appender(string $obj2): array
    {
        $arr = [];
        foreach ($this->records as $key => $obj) {
            if (rand(0, 1)) {
                $obj = new $obj2;
            }
            $arr[$key] = $obj;
        }

        return $arr;
    }
}