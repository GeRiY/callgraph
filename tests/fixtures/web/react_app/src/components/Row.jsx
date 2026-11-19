import PropTypes from 'prop-types';

export default function Row({ label, value, unit }) {
    return (
        <li>
            {label}: {value}
            {unit}
        </li>
    );
}

Row.propTypes = {
    label: PropTypes.string.isRequired,
    value: PropTypes.number,
    unit: PropTypes.string,
};

Row.defaultProps = { unit: 'pc' };
