!function() {
  let textarea = document.getElementById('weather-frame');
  if (textarea) {
    if (textarea.getAttribute('height')) {
      textarea.setAttribute('height', Math.floor(window.innerHeight * 750 / 856.));
    }
    if (textarea.getAttribute('width')) {
      textarea.setAttribute('width', Math.floor(window.innerWidth * 850 / 1105.));
    }
  }
}();
function updateLocation() {
  let current_location = "";
  if(document.getElementById('locationForm').value != "") {
    current_location = document.getElementById('locationForm').value;
    document.getElementById('locationForm').value = "";
    return current_location;
  } else if ("geolocation" in navigator) {
    navigator.geolocation.getCurrentPosition(
    function success(position) {
      current_location = "lat=" + position.coords.latitude + "&lon=" + position.coords.longitude;
    },
    function error(error_message) {
      // for when getting location results in an error
      console.error('No location retreived', error_message)
    });
  }
  return current_location;
}
