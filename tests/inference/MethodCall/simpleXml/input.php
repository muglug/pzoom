<?php
$xml = new SimpleXMLElement("<a><b></b></a>");
$a = $xml->asXML();
$b = $xml->asXML("foo.xml");
