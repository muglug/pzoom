<?php
/** @var array{
 *			address_components: list<array{
 *				long_name: string,
 *				short_name: string,
 *				types: list<("accounting"|"administrative_area_level_1"|"administrative_area_level_2"|"administrative_area_level_3"|
 *		"administrative_area_level_4"|"administrative_area_level_5"|"airport"|"amusement_park"|"art_gallery"|"bar"|"bus_station"|"cafe"|
 *		"campground"|"car_rental"|"cemetery"|"colloquial_area"|"continent"|"country"|"courthouse"|"embassy"|"establishment"|"finance"|
 *		"floor"|"food"|"funeral_home"|"general_contractor"|"gym"|"health"|"hospital"|"intersection"|"lawyer"|"light_rail_station"|
 *		"local_government_office"|"locality"|"lodging"|"moving_company"|"museum"|"natural_feature"|"neighborhood"|"night_club"|"park"|
 *		"parking"|"plus_code"|"point_of_interest"|"police"|"political"|"post_box"|"post_office"|"postal_code"|"postal_code_prefix"|
 *		"postal_code_suffix"|"postal_town"|"premise"|"real_estate_agency"|"restaurant"|"route"|"rv_park"|"school"|"spa"|"storage"|"store"|
 *		"street_address"|"street_number"|"sublocality"|"sublocality_level_1"|"sublocality_level_2"|"sublocality_level_3"|
 *		"sublocality_level_4"|"sublocality_level_5"|"subpremise"|"subway_station"|"tourist_attraction"|"town_square"|"train_station"|
 *		"transit_station"|"travel_agency"|"university"|"ward"|"zoo")>
 *			}>,
 *			formatted_address: string,
 *			geometry: array{
 *				location: array{ lat: float, lng: float },
 *				location_type: string,
 *				viewport: array{
 *					northeast: array{ lat: float, lng: float },
 *					southwest: array{ lat: float, lng: float }
 *				}
 *			},
 *			partial_match: bool,
 *			types: list<string>
 * }
 */
$data = [];
$cmp = [];
foreach ($data["address_components"] as $component) {
    foreach ($component["types"] as $type) {
        $cmp[$type] = $component["long_name"];
    }
}

if (!\array_key_exists("locality", $cmp)) {
    $cmp["locality"] = "";
}

if (!\array_key_exists("administrative_area_level_1", $cmp)) {
    $cmp["administrative_area_level_1"] = "";
}
if ($cmp["administrative_area_level_1"] === "test") {
    $cmp["administrative_area_level_1"] = "";
}
