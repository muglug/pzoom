<?php //--taint-analysis
class Wdb {
    /**
      * @psalm-pure
      *
      * @param string $text
      * @return string
      * @psalm-flow ($text) -> return
      */
    public function esc_like($text) {}

    /**
      * @param string $query
      * @return int|bool
      *
      * @psalm-taint-sink sql $query
      */
    public function query($query){}
}

$wdb = new Wdb();

$order = $wdb->esc_like($_GET["order"]);
$res = $wdb->query("SELECT blah FROM tablea ORDER BY ". $order. " DESC");
