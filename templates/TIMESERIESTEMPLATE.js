// Set the dimensions of the canvas / graph
let margin_NAME = {top: 30, right: 20, bottom: 30, left: 50};
let width_NAME = 600 - margin_NAME.left - margin_NAME.right;
let height_NAME = 270 - margin_NAME.top - margin_NAME.bottom;

// Parse the date / time
let parseDateTime_NAME = d3.timeParse("%Y-%m-%dT%H:%M:%S");

// Set the ranges
let x_NAME = d3.scaleTime().range([0, width_NAME]);
let y_NAME = d3.scaleLinear().range([height_NAME, 0]);

// Define the axes
let xAxis_NAME = d3.axisBottom(x_NAME).ticks(5);

let yAxis_NAME = d3.axisLeft(y_NAME).ticks(5);

// Define the line
let valueline_NAME = d3.line()
    .x(function(d) { return x_NAME(d[0]); })
    .y(function(d) { return y_NAME(d[1]); });
    
// Adds the svg canvas
let svg_NAME = d3.select("body")
    .append("svg")
        .attr("width", width_NAME + margin_NAME.left + margin_NAME.right)
        .attr("height", height_NAME + margin_NAME.top + margin_NAME.bottom)
    .append("g")
        .attr("transform",
              "translate(" + margin_NAME.left + "," + margin_NAME.top + ")")
    .on("mousemove touchmove", handleMouseOverData_NAME);

svg_NAME.append("text")      // text label for chart Title
        .attr("x", width_NAME / 2 )
        .attr("y", 0 - (margin_NAME.top/2))
        .style("text-anchor", "middle")
		.style("font-size", "16px")
        .style("text-decoration", "underline")
        .text("EXAMPLETITLE");

svg_NAME.append("text")      // text label for the x-axis
        .attr("x", width_NAME / 2 )
        .attr("y",  height_NAME + margin_NAME.bottom)
        .style("text-anchor", "middle")
        .text("XAXIS");

svg_NAME.append("text")      // text label for the y-axis
        .attr("y",30 - margin_NAME.left)
        .attr("x",50 - (height_NAME / 2))
        .attr("transform", "rotate(-90)")
        .style("text-anchor", "end")
        .style("font-size", "16px")
        .text("YAXIS");

// Get the data
let data_NAME =
    DATA;
data_NAME.forEach(function(d) {
    d[0] = parseDateTime_NAME(d[0]);
});

let xmax_NAME = d3.max(data_NAME, function(d) {return d[0]});
let xmin_NAME = d3.min(data_NAME, function(d) {return d[0]});
let ymax_NAME = d3.max(data_NAME, function(d) {return d[1]});
let ymin_NAME = d3.min(data_NAME, function(d) {return d[1]});

ymax_NAME = ymax_NAME + 0.1 * Math.abs(ymax_NAME);
ymin_NAME = ymin_NAME - 0.1 * Math.abs(ymin_NAME);

x_NAME.domain(d3.extent(data_NAME, function(d) {return d[0]; }));
y_NAME.domain([ymin_NAME, ymax_NAME]);

svg_NAME.append("path").attr("class", "line").attr("d", valueline_NAME(data_NAME));

svg_NAME.append("g")
      .attr("class", "x axis")
      .attr("transform", "translate(0," + height_NAME + ")")
      .call(xAxis_NAME)
	    .selectAll(".tick text")
      .call(wrap, 35);

svg_NAME.append("g").attr("class", "yaxis").call(yAxis_NAME);

function wrap(text, width_NAME) {
    text.each(function() {
      let text = d3.select(this),
          words = text.text().split(/\s+/).reverse(),
          word,
          line = [],
          lineNumber = 0,
          lineHeight = 1.1, // ems
          y = text.attr("y"),
          dy = parseFloat(text.attr("dy")),
          tspan = text.text(null).append("tspan").attr("x", 0).attr("y", y).attr("dy", dy + "em");
      while (word = words.pop()) {
        line.push(word);
        tspan.text(line.join(" "));
        if (tspan.node().getComputedTextLength() > width_NAME) {
          line.pop();
          tspan.text(line.join(" "));
          line = [word];
          tspan = text.append("tspan").attr("x", 0).attr("y", y).attr("dy", ++lineNumber * lineHeight + dy + "em").text(word);
        }
      }
    });
}

let rule_NAME = svg_NAME.append("g")
    .append("line")
      .attr("y1", y_NAME(ymin_NAME))
      .attr("y2", y_NAME(ymax_NAME))
      .attr("stroke", "black");

function handleMouseOverData_NAME() {
    let d = d3.mouse(this)
    let date = x_NAME.invert(d[0]);
    let heartrate = y_NAME.invert(d[1]);

    rule_NAME.attr("transform", `translate(${d[0]}, 0)`);

    svg_NAME.property("value", date).dispatch("input");
    d3.event.preventDefault();

    let data_date = d3.select('#data_date_NAME');
    if (data_date) {
        data_date.remove();
    }
    let data_heartrate = d3.select('#data_heartrate_NAME');
    if (data_heartrate) {
        data_heartrate.remove();
    }

    svg_NAME.append('text')
        .attr("id", 'data_date_NAME')
        .attr("x", function() {return x_NAME(xmin_NAME) + 30;})
        .attr("y", function() {return y_NAME(ymax_NAME) + 15;})
        .text(function() {return date;});
    svg_NAME.append('text')
        .attr("id", 'data_heartrate_NAME')
        .attr("x", function() {return x_NAME(xmin_NAME) + 30;})
        .attr("y", function() {return y_NAME(ymax_NAME) + 30;})
        .text(function() {return heartrate;});
}
