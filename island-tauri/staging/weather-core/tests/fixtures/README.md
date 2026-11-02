# Hand-written fixtures, not captured responses

All JSON here is hand-written from the official documentation, with invented
weather/time values and an additional artificial minimal geocoding record.
They are NOT recordings or evidence of a live network call.

- forecast.json: https://open-meteo.com/en/docs#json_return_object and
  #hourly_parameter_definition. Uses documented current variables and metadata.
  The extracted docs' static JSON example shows hourly_units, not current_units;
  the current_units spelling is part of the requested contract and corroborated
  by an indexed official API response, not explicitly shown in the static docs.
- geocode.json: https://open-meteo.com/en/docs/geocoding-api#json_return_object.
  Berlin is adapted from the documentation's example; the second entry is
  deliberately synthetic to test omitted optional fields and boundary coords.
- error.json: https://open-meteo.com/en/docs/geocoding-api#errors.

Tests also hand-build altered JSON values to exercise invalid/missing fields.
See ../../VERIFICATION.md for the complete documentation verification limits.
