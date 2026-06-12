<?php //--taint-analysis
class Wdb {
    /**
      * @psalm-pure
      *
      * @param string $text
      * @return string
      * @psalm-flow ($text) -> return
      */
    public static function esc_like($text) {}

    /**
      * @param string $query
      * @return int|bool
      *
      * @psalm-taint-sink sql $query
      */
    public static function query($query){}
}

$order = Wdb::esc_like($_GET["order"]);
$res = Wdb::query("SELECT blah FROM tablea ORDER BY ". $order. " DESC");
